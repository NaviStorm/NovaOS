#include <stdint.h>
#include <stdio.h>
#include <sys/utsname.h>
#include <time.h>

int main(void) {
    struct utsname data;
    const int iterations = 20000;
    uint64_t start = clock_gettime_nsec_np(CLOCK_UPTIME_RAW);
    for (int i = 0; i < iterations; ++i) {
        if (uname(&data) != 0) return 1;
    }
    printf("%.2f\n", (double)(clock_gettime_nsec_np(CLOCK_UPTIME_RAW) - start) / iterations);
    return 0;
}
