
impl Login {
 pub(super) fn membership_probe(&mut self,fs:&mut nova_cli::fileapi::Client,args:&[&str])->rt::Result<u64> {
  let [phase,object,g,h]=args else{return Err(rt::BAD_ARGUMENT)};
  if !matches!(*phase,"avant"|"apres"|"reprise"){return Err(rt::BAD_ARGUMENT);}
  let object:u64=object.parse().map_err(|_|rt::BAD_ARGUMENT)?;
  let hex=|s:&str|->rt::Result<[u8;16]>{if s.len()!=32{return Err(rt::BAD_ARGUMENT);}let mut a=[0;16];for i in 0..16{a[i]=u8::from_str_radix(&s[2*i..2*i+2],16).map_err(|_|rt::BAD_ARGUMENT)?;}Ok(a)};
  let groups=[[0;16],hex(g)?,hex(h)?];
  for (i,group) in groups.iter().enumerate(){
   let mut q=[0;128];q[..8].copy_from_slice(b"NXMGDAT1");q[8..40].copy_from_slice(&self.session.ok_or(rt::DENIED)?);q[40..48].copy_from_slice(&object.to_le_bytes());q[48..64].copy_from_slice(group);
   q[64..72].copy_from_slice(&self.membership_keys[i].to_le_bytes());q[72]=if *phase=="apres"{2}else{1};q[73]=if i==0{1}else{3};
   let tag=fs.membership_begin(&q)?;
   let mut decision=[0;88];decision[..80].copy_from_slice(&q[8..88]);decision[80..].copy_from_slice(&tag.to_le_bytes());
   let a=self.request(b"NSHMGVF1",&decision)?;
   let data=fs.membership_end(tag)?;let status=u64::from_le_bytes(data[8..16].try_into().unwrap());
   let expected=if *phase!="avant" && i==1{rt::REVOKED}else{0};
   if status!=expected || data[8..16]!=a[8..16]{return Err(rt::REFUSED);}
   if status==0 {
    let mut expected_bytes=*b"A\tB\tC\nD\tE\tF\n";
    expected_bytes[0]=if i==0 {if *phase=="avant"{b'A'}else{groups[2][0]^if *phase=="reprise"{0x80}else{0}}}else{group[0]^if *phase=="apres"{0x80}else{0}};
    if data[24..32]!=12u64.to_le_bytes() || data[32..44]!=expected_bytes || data[52..]!=[0;76]{return Err(rt::REFUSED);}
    self.membership_keys[i]=u64::from_le_bytes(data[16..24].try_into().unwrap());
   }else if data[16..]!=[0;112]{return Err(rt::REFUSED);}
   let line=alloc::format!("NARM Q1 phase={} origine={} native={} octets={} revision={}\n",phase,["Direct","G","H"][i],status,if status==0{12}else{0},u64::from_le_bytes(data[44..52].try_into().unwrap()));
   super::console::write(super::tty::OUTPUT,&line)?;
  }
  Ok(0)
 }
}
