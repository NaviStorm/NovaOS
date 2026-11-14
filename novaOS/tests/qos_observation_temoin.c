/* Témoin de laboratoire. API pthread et horloges publiques du SDK macOS. */
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

static int changer;
static int erreur;

static uint64_t maintenant(clockid_t horloge) {
    struct timespec t;
    if (clock_gettime(horloge, &t)) {
        perror("clock_gettime");
        exit(2);
    }
    return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}

static void *travail(void *argument) {
    (void)argument;
    erreur = pthread_setname_np("CPU 0/TCG");
    if (erreur) return NULL;
    uint64_t tid = 0;
    erreur = pthread_threadid_np(NULL, &tid);
    if (erreur) return NULL;
    printf("%d %llu\n", getpid(), (unsigned long long)tid);
    fflush(stdout);
    volatile unsigned long calcul = 0;
    for (int n = 0; n < 120; n++) {
        if (n == 0 || (changer && n % 5 == 0)) {
            qos_class_t classe = QOS_CLASS_UTILITY;
            if (changer) classe = (n / 5) % 2 ? QOS_CLASS_BACKGROUND : QOS_CLASS_USER_INITIATED;
            erreur = pthread_set_qos_class_self_np(classe, 0);
            if (erreur) return NULL;
        }
        uint64_t debut = maintenant(CLOCK_MONOTONIC);
        do {
            for (int i = 0; i < 65536; i++) calcul += i;
        } while (maintenant(CLOCK_MONOTONIC) - debut < 100000000);
        usleep(100000);
    }
    return NULL;
}

int main(int argc, char **argv) {
    (void)argv;
    changer = argc > 1;
    pthread_t thread;
    if (pthread_create(&thread, NULL, travail, NULL)) return 2;
    if (pthread_join(thread, NULL)) return 3;
    if (erreur) fprintf(stderr, "API pthread refusée %d\n", erreur);
    return erreur ? 4 : 0;
}
