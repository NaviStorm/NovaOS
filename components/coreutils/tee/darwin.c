/* Adaptateur Darwin. Le point d'entrée C conserve les signaux hérités. */
#include <errno.h>
#include <fcntl.h>
#include <langinfo.h>
#include <locale.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <strings.h>
#include <sys/stat.h>
#include <unistd.h>

extern int tee_main(int argc, const char *const argv[]);
bool tee_utf8(void) { return strcasecmp(nl_langinfo(CODESET), "UTF-8") == 0; }
int main(int argc, char **argv) {
  setlocale(LC_ALL, "");
  return tee_main(argc, (const char *const *)argv);
}

void tee_signals(bool interrupt, bool pipe_errors) {
  if (interrupt)
    signal(SIGINT, SIG_IGN);
  if (pipe_errors)
    signal(SIGPIPE, SIG_IGN);
}
int tee_open(const char *name, bool append) {
  int fd = open(name, O_WRONLY | O_CREAT | (append ? O_APPEND : O_TRUNC), 0666);
  if (fd < 0)
    return -errno;
  /* Une ouverture ne doit jamais réattribuer une entrée/sortie standard
   * absente. */
  if (fd <= 2) {
    int moved = fcntl(fd, F_DUPFD_CLOEXEC, 3);
    int err = errno;
    close(fd);
    return moved < 0 ? -err : moved;
  }
  return fd;
}
int tee_read(uint8_t *p, size_t len) {
  ssize_t n;
  do {
    n = read(0, p, len);
  } while (n < 0 && errno == EINTR);
  return n < 0 ? -errno : (int)n;
}
static int wait_events(struct pollfd *fds, nfds_t count) {
  int rc;
  do {
    rc = poll(fds, count, -1);
  } while (rc < 0 && errno == EINTR);
  return rc < 0 ? -errno : 0;
}
int tee_watch(int fd) {
  struct stat s;
  if (fstat(0, &s) < 0 || S_ISREG(s.st_mode) || S_ISBLK(s.st_mode))
    return 0;
  if (fstat(fd, &s) < 0 || !S_ISFIFO(s.st_mode))
    return 0;
  struct pollfd events[] = {{0, POLLIN | POLLRDBAND, 0}, {fd, POLLRDBAND, 0}};
  int rc = wait_events(events, 2);
  if (rc < 0)
    return rc;
  return !events[0].revents &&
         (events[1].revents & (POLLHUP | POLLERR | POLLNVAL));
}
int tee_write(int fd, const uint8_t *p, size_t len) {
  while (len) {
    ssize_t n = write(fd, p, len);
    if (n > 0) {
      p += n;
      len -= (size_t)n;
      continue;
    }
    if (!n)
      return -ENOSPC;
    if (errno == EINTR)
      continue;
    if (errno != EAGAIN && errno != EWOULDBLOCK)
      return -errno;
    struct pollfd out = {fd, POLLOUT, 0};
    int result = wait_events(&out, 1);
    if (result < 0)
      return result;
  }
  return 0;
}
int tee_close(int fd) {
  if (close(fd) == 0)
    return 0;
  // Si stdout était déjà fermé et qu'aucun octet n'a été écrit, GNU réussit.
  return fd == STDOUT_FILENO && errno == EBADF ? 0 : -errno;
}
bool tee_broken(int code) { return code == EPIPE; }
