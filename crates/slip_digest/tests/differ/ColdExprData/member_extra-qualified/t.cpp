#include "side.h"
// §7.4 ColdExprData `member_extra-qualified`: a qualified call of a
// virtual function is not dispatched. Written `virtual`; an overrider
// declared without `virtual` (kiln_sema class/virtual/mod.rs:160); a
// virtual destructor.
struct Bs { virtual int f(); virtual ~Bs(); };
struct Ov : Bs { int f(); };
Bs *pb; Ov *po;
#ifdef B
int v = pb->f();
int vo = po->f();
int vd = (pb->~Bs(), 0);
#else
int v = pb->Bs::f();
int vo = po->Ov::f();
int vd = (pb->Bs::~Bs(), 0);
#endif
