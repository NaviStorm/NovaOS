/* Valeurs de l'hôte demandées par les tests GNU sort sur Darwin. */
#include <float.h>
#include <inttypes.h>
#include <limits.h>
#include <stdio.h>
#include <stdint.h>

int main(void) {
    printf("INT_MAX=%d\nSIZE_MAX=%zu\nUINTMAX_MAX=%" PRIuMAX "\n",
           INT_MAX, SIZE_MAX, UINTMAX_MAX);
    /* Unsigned types have power-of-two moduli. Print the mathematical successor
       in decimal without overflowing the largest type on this machine. */
    char number[64];
    int n = snprintf(number, sizeof number, "%" PRIuMAX, UINTMAX_MAX);
    for (int i = n-1; i >= 0; --i) {
        if (number[i] < '9') { ++number[i]; break; }
        number[i] = '0';
    }
    printf("UINTMAX_OFLOW=%s\n", number);
    printf("FLT_MIN=%.*e\nFLT_MAX=%.*e\n", FLT_DECIMAL_DIG, (double)FLT_MIN, FLT_DECIMAL_DIG, (double)FLT_MAX);
    printf("DBL_MIN=%.*e\nDBL_MAX=%.*e\n", DBL_DECIMAL_DIG, DBL_MIN, DBL_DECIMAL_DIG, DBL_MAX);
    printf("LDBL_MIN=%.*Le\nLDBL_MAX=%.*Le\n", LDBL_DECIMAL_DIG, LDBL_MIN, LDBL_DECIMAL_DIG, LDBL_MAX);
    return 0;
}
