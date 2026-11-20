# Разбор зависимостей бинарей: PE (Windows), Mach-O (macOS), ELF (Linux).
#
# Нужен затем, что однажды в пакет не попала libunicorn.dll, и у всех, кто
# ставил Signum с нуля, молча переставали работать снятые с продажи приложения.
# Список имён такое не ловит — ловит только разбор таблиц импорта.
#
# Использование:  python inspect-binary-deps.py <файл> [файл ...]
#
import struct, sys, os, json

# ---------------- PE ----------------
def pe_imports(path):
    with open(path,'rb') as f:
        data=f.read()
    if data[:2]!=b'MZ': return None
    e_lfanew=struct.unpack_from('<I',data,0x3c)[0]
    if data[e_lfanew:e_lfanew+4]!=b'PE\0\0': return None
    coff=e_lfanew+4
    nsec=struct.unpack_from('<H',data,coff+2)[0]
    optsz=struct.unpack_from('<H',data,coff+16)[0]
    opt=coff+20
    magic=struct.unpack_from('<H',data,opt)[0]
    pe32p = (magic==0x20b)
    imagebase = struct.unpack_from('<Q',data,opt+24)[0] if pe32p else struct.unpack_from('<I',data,opt+28)[0]
    ddoff = opt + (112 if pe32p else 96)
    nrva = struct.unpack_from('<I',data,opt+(108 if pe32p else 92))[0]
    imp_rva,imp_sz = struct.unpack_from('<II',data,ddoff+1*8)
    delay_rva,delay_sz = struct.unpack_from('<II',data,ddoff+13*8) if nrva>13 else (0,0)
    secs=[]
    so=opt+optsz
    for i in range(nsec):
        b=so+i*40
        name=data[b:b+8].rstrip(b'\0').decode('latin1')
        vsz,va,rsz,ra = struct.unpack_from('<IIII',data,b+8)
        secs.append((name,va,vsz,ra,rsz))
    def r2o(rva):
        for name,va,vsz,ra,rsz in secs:
            if va <= rva < va+max(vsz,rsz):
                o = ra + (rva-va)
                if 0 <= o < len(data): return o
        return None
    def cstr(off):
        if off is None: return None
        end=data.find(b'\0',off)
        if end<0: return None
        return data[off:end].decode('latin1')
    names=[]
    if imp_rva:
        o=r2o(imp_rva)
        if o is not None:
            while True:
                ent=data[o:o+20]
                if len(ent)<20 or ent==b'\0'*20: break
                oft,ts,fc,nrva2,fthunk=struct.unpack('<IIIII',ent)
                if nrva2==0: break
                n=cstr(r2o(nrva2))
                if n: names.append(['import',n])
                o+=20
    if delay_rva:
        o=r2o(delay_rva)
        if o is not None:
            while True:
                ent=data[o:o+32]
                if len(ent)<32 or ent==b'\0'*32: break
                attrs,nrva2=struct.unpack_from('<II',ent,0)
                if nrva2==0: break
                rva = nrva2 if (attrs & 1) else (nrva2 - imagebase)
                n=cstr(r2o(rva))
                if n: names.append(['delay',n])
                o+=32
    return names

# ---------------- Mach-O ----------------
LC_LOAD_DYLIB=0xc; LC_LOAD_WEAK_DYLIB=0x80000018; LC_REEXPORT_DYLIB=0x8000001f
LC_LOAD_UPWARD_DYLIB=0x80000023; LC_RPATH=0x8000001c; LC_ID_DYLIB=0xd
LC_LAZY_LOAD_DYLIB=0x20
CPU={16777228:'arm64',16777223:'x86_64',7:'i386',12:'arm'}

def macho_one(data, base, be, is64):
    en='>' if be else '<'
    ncmds=struct.unpack_from(en+'I',data,base+16)[0]
    off=base+(32 if is64 else 28)
    dylibs=[]; rpaths=[]; ident=None
    for _ in range(ncmds):
        if off+8>len(data): break
        cmd,cmdsize=struct.unpack_from(en+'II',data,off)
        if cmdsize==0: break
        if cmd in (LC_LOAD_DYLIB,LC_LOAD_WEAK_DYLIB,LC_REEXPORT_DYLIB,LC_LOAD_UPWARD_DYLIB,LC_LAZY_LOAD_DYLIB,LC_ID_DYLIB):
            stroff=struct.unpack_from(en+'I',data,off+8)[0]
            s=data[off+stroff:off+cmdsize].split(b'\0')[0].decode('latin1')
            if cmd==LC_ID_DYLIB: ident=s
            else: dylibs.append([hex(cmd),s])
        elif cmd==LC_RPATH:
            stroff=struct.unpack_from(en+'I',data,off+8)[0]
            rpaths.append(data[off+stroff:off+cmdsize].split(b'\0')[0].decode('latin1'))
        off+=cmdsize
    cputype=struct.unpack_from(en+'i',data,base+4)[0]
    return {'cpu':CPU.get(cputype,cputype),'id':ident,'dylibs':dylibs,'rpaths':rpaths}

def macho(path):
    with open(path,'rb') as f: data=f.read()
    m=data[:4]
    slices=[]
    if m in (b'\xca\xfe\xba\xbe', b'\xca\xfe\xba\xbf'):
        nfat=struct.unpack_from('>I',data,4)[0]
        is64fat = (m==b'\xca\xfe\xba\xbf')
        for i in range(nfat):
            if is64fat:
                b=8+i*32; off=struct.unpack_from('>Q',data,b+8)[0]
            else:
                b=8+i*20; off=struct.unpack_from('>I',data,b+8)[0]
            slices.append(off)
    elif m in (b'\xcf\xfa\xed\xfe',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xfe\xed\xfa\xce'):
        slices=[0]
    else:
        return None
    out=[]
    for s in slices:
        mm=data[s:s+4]
        be = mm in (b'\xfe\xed\xfa\xcf',b'\xfe\xed\xfa\xce')
        is64 = mm in (b'\xcf\xfa\xed\xfe',b'\xfe\xed\xfa\xcf')
        out.append(macho_one(data,s,be,is64))
    return out

# ---------------- ELF ----------------
EM={62:'x86_64',183:'aarch64',3:'i386',40:'arm'}
def elf(path):
    with open(path,'rb') as f: data=f.read()
    if data[:4]!=b'\x7fELF': return None
    cls=data[4]; en='<' if data[5]==1 else '>'
    is64=cls==2
    if is64:
        e_phoff=struct.unpack_from(en+'Q',data,0x20)[0]
        e_phentsize=struct.unpack_from(en+'H',data,0x36)[0]
        e_phnum=struct.unpack_from(en+'H',data,0x38)[0]
    else:
        e_phoff=struct.unpack_from(en+'I',data,0x1c)[0]
        e_phentsize=struct.unpack_from(en+'H',data,0x2a)[0]
        e_phnum=struct.unpack_from(en+'H',data,0x2c)[0]
    machine=struct.unpack_from(en+'H',data,0x12)[0]
    dyn=None; interp=None; loads=[]
    for i in range(e_phnum):
        b=e_phoff+i*e_phentsize
        p_type=struct.unpack_from(en+'I',data,b)[0]
        if is64:
            p_offset=struct.unpack_from(en+'Q',data,b+8)[0]
            p_vaddr=struct.unpack_from(en+'Q',data,b+16)[0]
            p_filesz=struct.unpack_from(en+'Q',data,b+32)[0]
            p_memsz=struct.unpack_from(en+'Q',data,b+40)[0]
        else:
            p_offset=struct.unpack_from(en+'I',data,b+4)[0]
            p_vaddr=struct.unpack_from(en+'I',data,b+8)[0]
            p_filesz=struct.unpack_from(en+'I',data,b+16)[0]
            p_memsz=struct.unpack_from(en+'I',data,b+20)[0]
        if p_type==1: loads.append((p_vaddr,p_offset,p_filesz,p_memsz))
        if p_type==2: dyn=(p_offset,p_filesz)
        if p_type==3: interp=data[p_offset:p_offset+p_filesz].split(b'\0')[0].decode('latin1')
    needed=[]; rpath=[]; soname=None
    if dyn:
        def v2o(v):
            for va,off,fsz,msz in loads:
                if va<=v<va+msz: return off+(v-va)
            return None
        off,sz=dyn
        entsz=16 if is64 else 8
        strtab=None; ents=[]
        n=sz//entsz
        for i in range(n):
            tag,val=struct.unpack_from(en+('qQ' if is64 else 'iI'),data,off+i*entsz)
            if tag==0: break
            ents.append((tag,val))
            if tag==5: strtab=val
        so=v2o(strtab) if strtab is not None else None
        def s(x):
            if so is None: return '?'
            e=data.find(b'\0',so+x)
            return data[so+x:e].decode('latin1')
        for tag,val in ents:
            if tag==1: needed.append(s(val))
            elif tag==14: soname=s(val)
            elif tag in (15,29): rpath.append([('RPATH' if tag==15 else 'RUNPATH'),s(val)])
    return {'machine':EM.get(machine,machine),'interp':interp,'needed':needed,'rpath':rpath,'soname':soname,'static':dyn is None}

def analyze(p):
    try:
        with open(p,'rb') as f: m=f.read(4)
    except Exception as e:
        return {'err':str(e)}
    if m[:2]==b'MZ':
        return {'fmt':'PE','imports':pe_imports(p)}
    if m[:4]==b'\x7fELF':
        return {'fmt':'ELF','elf':elf(p)}
    r=macho(p)
    if r: return {'fmt':'MachO','slices':r}
    return {'fmt':'other','magic':m.hex()}

if __name__=='__main__':
    out={}
    for p in sys.argv[1:]:
        out[p]=analyze(p)
    print(json.dumps(out,indent=1))
