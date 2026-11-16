/* Adaptateur C du moteur QSE/TRE. Toutes les opérations système sont déléguées. */
#include "qse/lib/sed/sed-prv.h"
#include <qse/cmn/chr.h>
#include <qse/cmn/mem.h>
#include <stddef.h>

extern void *sed_alloc(size_t);
extern void *sed_resize(void *, size_t);
extern void sed_free(void *);
extern long sed_io(void *, int, int, const char *, size_t *, char *, size_t);
extern void sed_error(void *, const char *);
static void *allocate(qse_mmgr_t *m, size_t n) { return sed_alloc(n); }
static void *resize(qse_mmgr_t *m, void *p, size_t n) { return sed_resize(p, n); }
static void release(qse_mmgr_t *m, void *p) { sed_free(p); }
static qse_mmgr_t memory = {allocate, resize, release, NULL};

void *qse_memcpy(void *d, const void *s, size_t n) { return __builtin_memcpy(d,s,n); }
void *qse_memmove(void *d, const void *s, size_t n) { return __builtin_memmove(d,s,n); }
void *qse_memset(void *d, int v, size_t n) { return __builtin_memset(d,v,n); }
int qse_memcmp(const void *a, const void *b, size_t n) { return __builtin_memcmp(a,b,n); }

int qse_ismctype(int c, qse_mctype_t type) {
    int lower = c >= 'a' && c <= 'z', upper = c >= 'A' && c <= 'Z';
    int digit = c >= '0' && c <= '9', alpha = lower || upper;
    switch (type) {
        case QSE_CTYPE_ALNUM: return alpha || digit;
        case QSE_CTYPE_ALPHA: return alpha;
        case QSE_CTYPE_BLANK: return c == ' ' || c == '\t';
        case QSE_CTYPE_CNTRL: return (unsigned)c < 32 || c == 127;
        case QSE_CTYPE_DIGIT: return digit;
        case QSE_CTYPE_GRAPH: return c >= 33 && c < 127;
        case QSE_CTYPE_LOWER: return lower;
        case QSE_CTYPE_PRINT: return c >= 32 && c < 127;
        case QSE_CTYPE_PUNCT: return c >= 33 && c < 127 && !alpha && !digit;
        case QSE_CTYPE_SPACE: return c == ' ' || (c >= 9 && c <= 13);
        case QSE_CTYPE_UPPER: return upper;
        case QSE_CTYPE_XDIGIT: return digit || (c >= 'A' && c <= 'F') || (c >= 'a' && c <= 'f');
    }
    return 0;
}
int qse_tomctype(int c, qse_mctype_t type) {
    if (type == QSE_CTYPE_UPPER && c >= 'a' && c <= 'z') return c - 32;
    if (type == QSE_CTYPE_LOWER && c >= 'A' && c <= 'Z') return c + 32;
    return c;
}
int qse_mbsntoctype(const char *s, size_t n, qse_mctype_t *type) {
    static const char *const names[] = {"alnum","alpha","blank","cntrl","digit","graph",
                                        "lower","print","punct","space","upper","xdigit"};
    for (size_t i=0; i < sizeof(names)/sizeof(*names); i++) {
        if (qse_mbsxcmp(s,n,names[i]) == 0) { *type = (qse_mctype_t)(1+i); return 0; }
    }
    return -1;
}

struct context { const char *script; size_t left; void *adapter; };
static long script(qse_sed_t *sed,qse_sed_io_cmd_t op,qse_sed_io_arg_t *arg,char *buf,size_t n) {
    struct context *ctx = qse_sed_getxtn(sed);
    if (op != QSE_SED_IO_READ) return 0;
    if (n > ctx->left) n = ctx->left;
    if (n) { qse_memcpy(buf,ctx->script,n); ctx->script += n; ctx->left -= n; }
    return n;
}
static long transfer(qse_sed_t *sed,int out,int op,qse_sed_io_arg_t *arg,char *buf,size_t n) {
    struct context *ctx = qse_sed_getxtn(sed);
    size_t handle = op == QSE_SED_IO_OPEN ? 0 : (size_t)arg->handle;
    long result = sed_io(ctx->adapter,out,op,arg->path,&handle,buf,n);
    if (op == QSE_SED_IO_OPEN) arg->handle = (void *)handle;
    return result;
}
static long input(qse_sed_t *s,qse_sed_io_cmd_t op,qse_sed_io_arg_t *a,char *b,size_t n) {
    return transfer(s,0,op,a,b,n);
}
static long output(qse_sed_t *s,qse_sed_io_cmd_t op,qse_sed_io_arg_t *a,char *b,size_t n) {
    return transfer(s,1,op,a,b,n);
}

/* Pas d'E/S fichier avant la compilation de tout le programme. */
qse_sed_t *sed_compile(const char *src,size_t n,int options,size_t width,void *adapter) {
    qse_sed_t *s = qse_sed_open(&memory,sizeof(struct context),NULL);
    if (!s) { sed_error(adapter,"insufficient memory"); return NULL; }
    struct context *ctx = qse_sed_getxtn(s);
    *ctx = (struct context){src,n,adapter};
    int trait = QSE_SED_ENSURENL | QSE_SED_SAMELINE | QSE_SED_EXTENDEDADR;
    if (options & 1) trait |= QSE_SED_QUIET;
    if (options & 2) trait |= QSE_SED_EXTENDEDREX;
    s->list_width = width;
    s->sandbox = options & 4;
    s->strict_posix = options & 16;
    s->native = options & 32;
    if (s->strict_posix) {trait &= ~QSE_SED_EXTENDEDADR; trait |= QSE_SED_STRICT;}
    s->record_sep = (options & 8) ? '\0' : '\n';
    qse_sed_setopt(s,QSE_SED_TRAIT,&trait);
    if (qse_sed_comp(s,script) < 0) {
        sed_error(adapter,qse_sed_geterrmsg(s)); qse_sed_close(s); return NULL;
    }
    return s;
}
int sed_execute(qse_sed_t *s,void *adapter) {
    struct context *ctx = qse_sed_getxtn(s);
    ctx->adapter = adapter;
    s->quit_code = -1;
    if (qse_sed_exec(s,input,output) < 0) {
        sed_error(adapter,qse_sed_geterrmsg(s)); return -2;
    }
    return s->quit_code;
}


void *qse_membyte(const void *s,int val,size_t n) {
    const unsigned char *bytes=s;
    for (size_t i=0;i<n;i++) if (bytes[i]==(unsigned char)val) return (void *)(bytes+i);
    return NULL;
}
extern long sed_system(void *,const char *,size_t,char **);
long nova_sed_command(qse_sed_t *s,const char *cmd,size_t len,char **result) {
    struct context *ctx=qse_sed_getxtn(s);
    return sed_system(ctx->adapter,cmd,len,result);
}
extern long sed_filename(void *,char **);
long nova_sed_filename(qse_sed_t *s,char **result) {
    struct context *ctx=qse_sed_getxtn(s);
    return sed_filename(ctx->adapter,result);
}
