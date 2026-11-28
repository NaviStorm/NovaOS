/* Témoin sur un CD séparé. WinPE fournit Kernel32. Aucun pilote et aucun appel en écriture. */
typedef unsigned long DWORD;
typedef void *HANDLE;
__declspec(dllimport) DWORD GetEnvironmentVariableA(const char *, char *, DWORD);
__declspec(dllimport) HANDLE CreateFileA(const char *, DWORD, DWORD, void *, DWORD, DWORD, HANDLE);
__declspec(dllimport) int ReadFile(HANDLE, void *, DWORD, DWORD *, void *);
__declspec(dllimport) int CloseHandle(HANDLE);
__declspec(dllimport) __declspec(noreturn) void ExitProcess(DWORD);

static int lire(const char *nom, unsigned char *buf, DWORD longueur)
{
    HANDLE fichier = CreateFileA(nom, 0x80000000UL, 1, 0, 3, 0, 0);
    if (fichier == (HANDLE)-1)
        return 0;
    DWORD taille = 0;
    /* Un octet supplémentaire détecte les fichiers allongés. */
    int ok = ReadFile(fichier, buf, longueur + 1, &taille, 0) && taille == longueur;
    CloseHandle(fichier);
    return ok;
}

static unsigned char a[2049], b[2049];

void entree(void)
{
    char usb[40] = "?:\\BOOT0001\\";
    char cd[40] = "?:\\REF\\BOOT0001\\";
    const unsigned int debut_usb = sizeof("?:\\BOOT0001\\") - 1;
    const unsigned int debut_cd = sizeof("?:\\REF\\BOOT0001\\") - 1;
    char lecteur[4];
    if (GetEnvironmentVariableA("LOG", lecteur, sizeof lecteur) != 2 || lecteur[1] != ':')
        ExitProcess(1);
    usb[0] = lecteur[0];
    if (GetEnvironmentVariableA("REF", lecteur, sizeof lecteur) != 2 || lecteur[1] != ':')
        ExitProcess(2);
    cd[0] = lecteur[0];
    if (usb[0] == cd[0])
        ExitProcess(3);
    const char *noms[] = {"rapport.json", "JOURNAL.TXT", "VALIDES.SHA"};
    const DWORD tailles[] = {2048, 1024, 512};
    for (int slot = 1; slot <= 8; slot++) {
        usb[debut_usb - 2] = cd[debut_cd - 2] = (char)('0' + slot);
        for (int k = 0; k < 3; k++) {
            unsigned int j;
            for (j = 0; noms[k][j]; j++) {
                usb[debut_usb + j] = cd[debut_cd + j] = noms[k][j];
            }
            usb[debut_usb + j] = cd[debut_cd + j] = 0;
            if (!lire(usb, a, tailles[k]) || !lire(cd, b, tailles[k]))
                ExitProcess(4);
            for (DWORD pos = 0; pos < tailles[k]; pos++) {
                if (a[pos] != b[pos])
                    ExitProcess(5);
            }
        }
    }
    ExitProcess(0);
}
