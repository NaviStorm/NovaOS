/* MIT, ARC-027. PCRE2's code and match data stay in the worker process. */
#define PCRE2_CODE_UNIT_WIDTH 8
#include <pcre2.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    pcre2_code *code;
    pcre2_match_data *data;
} grep_perl;

grep_perl *grep_perl_compile(const uint8_t *pattern, size_t len, int utf,
                             int caseless, int *error) {
    *error = PCRE2_ERROR_NOMEMORY;
    grep_perl *p = calloc(1, sizeof *p);
    if (!p) return NULL;
    pcre2_compile_context *ctx = pcre2_compile_context_create(NULL);
    if (ctx) {
        size_t offset;
        pcre2_set_compile_extra_options(ctx, PCRE2_EXTRA_ASCII_BSD);
        uint32_t options = PCRE2_DOLLAR_ENDONLY | (caseless ? PCRE2_CASELESS : 0) |
            (utf ? PCRE2_UTF | PCRE2_UCP | PCRE2_MATCH_INVALID_UTF : 0);
        p->code = pcre2_compile(pattern, len, options, error, &offset, ctx);
        pcre2_compile_context_free(ctx);
        if (p->code) {
            /* On Darwin use the PCRE2 JIT when the installed backend supports it.
               pcre2_match falls back to its interpreter on unsupported patterns. */
            (void)pcre2_jit_compile(p->code, PCRE2_JIT_COMPLETE);
            p->data = pcre2_match_data_create_from_pattern(p->code, NULL);
            if (p->data) return p;
            *error = PCRE2_ERROR_NOMEMORY;
            pcre2_code_free(p->code);
        }
    }
    free(p);
    return NULL;
}

int grep_perl_find(grep_perl *p, const uint8_t *text, size_t len,
                   size_t start, int not_bol, size_t span[2]) {
    int result = pcre2_match(p->code, text, len, start, not_bol ? PCRE2_NOTBOL : 0, p->data, NULL);
    if (result == PCRE2_ERROR_NOMATCH) return 0;
    if (result < 0) return result;
    PCRE2_SIZE *ov = pcre2_get_ovector_pointer(p->data);
    span[0] = ov[0];
    span[1] = ov[1];
    return 1;
}

void grep_perl_free(grep_perl *p) {
    pcre2_match_data_free(p->data);
    pcre2_code_free(p->code);
    free(p);
}

int grep_perl_error(int code, uint8_t *out, size_t len) {
    if (code == PCRE2_ERROR_MATCHLIMIT) {
        const char msg[] = "exceeded PCRE's backtracking limit";
        if (len >= sizeof msg) { memcpy(out, msg, sizeof msg); return sizeof msg - 1; }
    }
    return pcre2_get_error_message(code, out, len);
}
