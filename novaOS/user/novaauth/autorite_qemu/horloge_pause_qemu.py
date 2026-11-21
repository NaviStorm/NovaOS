"""Horloge du banc : les suspensions QMP explicites ne sont pas du temps invité.

Aucune échéance ni horloge de NovaOS ou de l'autorité distante n'est modifiée.
Le timeout du socket hôte reste de 45 s de VM non suspendue.
"""
import select,ssl,threading,time

class Clock:
    def __init__(self,now=time.monotonic):
        self.now=now;self.lock=threading.Lock();self.started=None;self.total=0.0
    def pause(self):
        with self.lock:
            if self.started is None:self.started=self.now()
    def resume(self):
        with self.lock:
            if self.started is not None:
                self.total+=self.now()-self.started;self.started=None
    def active(self):
        with self.lock:
            now=self.now()
            return now-self.total-(now-self.started if self.started is not None else 0.0)
    def excluded(self):
        with self.lock:
            return self.total+(self.now()-self.started if self.started is not None else 0.0)

class Stream:
    def __init__(self,socket,clock,limit=45.0):
        self.socket=socket;self.clock=clock;self.limit=limit
        socket.setblocking(False)
    def __getattr__(self,name):return getattr(self.socket,name)
    def attempt(self,operation,deadline=None):
        if deadline is None:deadline=self.clock.active()+self.limit
        while True:
            try:return operation()
            except ssl.SSLWantReadError:read,write=[self.socket],[]
            except ssl.SSLWantWriteError:read,write=[],[self.socket]
            if self.clock.active()>=deadline:raise TimeoutError('TLS hôte : 45 s actives dépassées')
            select.select(read,write,[],0.05)
    def do_handshake(self):return self.attempt(self.socket.do_handshake)
    def recv(self,n):return self.attempt(lambda:self.socket.recv(n))
    def recv_into(self,buffer):return self.attempt(lambda:self.socket.recv_into(buffer))
    def sendall(self,data):
        view=memoryview(data);deadline=self.clock.active()+self.limit
        while view:
            n=self.attempt(lambda:self.socket.send(view),deadline)
            if n<=0:raise EOFError('fermeture TLS hôte pendant envoi')
            view=view[n:]
