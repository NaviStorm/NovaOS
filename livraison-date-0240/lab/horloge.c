/* Horloge du banc Darwin uniquement. Aucune fonction de réglage système
   n'est appelée. Le journal privé contient les valeurs reçues par le double. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

static struct timespec instant = {1709164800, 0};
__attribute__((constructor)) static void init_test(void) {
    const char *value = getenv("DATE_TEST_SECOND");
    if (value) instant.tv_sec = (time_t)atoll(value);
}
static int clock_read(clockid_t id, struct timespec *out) {
    if (id != CLOCK_REALTIME) return clock_gettime(id, out);
    *out = instant;
    if (!getenv("DATE_TEST_SECOND")) instant.tv_nsec += 1000;
    if (instant.tv_nsec >= 1000000000) { instant.tv_sec++; instant.tv_nsec = 0; }
    return 0;
}
static time_t seconds_read(time_t *out) { if (out) *out = instant.tv_sec; return instant.tv_sec; }
static int time_read(struct timeval *out, void *zone) {
    (void)zone;
    out->tv_sec = instant.tv_sec;
    out->tv_usec = instant.tv_nsec / 1000;
    return 0;
}
static int clock_write(clockid_t id, const struct timespec *value) {
    const char *log = getenv("DATE_TEST_RECORD");
    if (id != CLOCK_REALTIME || !log) { errno = EPERM; return -1; }
    if (getenv("DATE_TEST_DENY")) { errno = EPERM; return -1; }
    int fd = open(log, O_WRONLY|O_APPEND|O_CLOEXEC);
    if (fd < 0) return -1;
    instant = *value;
    int n = dprintf(fd, "%lld.%09ld\n", (long long)value->tv_sec, value->tv_nsec);
    int closed = close(fd);
    return n < 0 || closed < 0 ? -1 : 0;
}
static int time_write(const struct timeval *v, const struct timezone *zone) {
    (void)zone;
    const struct timespec t = {v->tv_sec, (long)v->tv_usec * 1000};
    return clock_write(CLOCK_REALTIME, &t);
}
#define REPLACE(f, orig) __attribute__((used, section("__DATA,__interpose"))) \
    static const struct { const void *replacement; const void *original; } \
    pair_##f = { (const void *)&f, (const void *)&orig }
REPLACE(clock_read,clock_gettime);
REPLACE(time_read,gettimeofday);
REPLACE(seconds_read,time);
REPLACE(clock_write,clock_settime);
REPLACE(time_write,settimeofday);
