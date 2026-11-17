"""Le raccord ajoute un appel typé, conserve les preuves et interdit le rejeu."""
from dataclasses import replace
import unittest
from test_reprise import Reprise
from reprise import Refus,HEAD_CHANGE,HEAD_RECEIPT,QUERY
from mutation_rpc import request

class Mutation(Reprise):
    # Les tests historiques hérités restent exécutés sans changer leurs attentes.
    def test_publication_signee_recu_perdu_et_reconciliation(self):
        peer=self.server.public_key().public_bytes_raw()
        state,_=self.api.observe_peer(*self.pair,b"N"*32,peer)
        op=b"O"*16;head=b"J"*32;packet=state.mutation(op,head)
        body=b"".join(self.pair)+self.old+state.packet()+op+head+self.server.sign(packet)
        for offset in [0,16,32,64,80,96,176,192]:
            bad=bytearray(body);bad[offset]^=1
            with self.assertRaises((Refus,ValueError)):request(self.path,self.authority,peer,self.pair,30,bytes(bad))
        receipt=request(self.path,self.authority,peer,self.pair,30,body)
        self.authority.public_key().verify(receipt,HEAD_RECEIPT+packet)
        with self.assertRaises(Refus):request(self.path,self.authority,peer,self.pair,30,body)
        nonce=b"F"*32
        recovered=request(self.path,self.authority,peer,self.pair,31,b"".join(self.pair)+self.old+op+nonce)
        self.assertEqual(recovered[:-64],packet+receipt)
        self.authority.public_key().verify(recovered[-64:],QUERY+nonce+packet+receipt)
        self.assertEqual(self.state(),(self.old,0,6,head))
        self.restart()
        self.assertEqual(request(self.path,self.authority,peer,self.pair,31,b"".join(self.pair)+self.old+op+nonce),recovered)
        for kind,data in [(30,body[:-1]),(31,body),(32,body)]:
            with self.assertRaises(Refus):request(self.path,self.authority,peer,self.pair,kind,data)

if __name__=="__main__":unittest.main()
