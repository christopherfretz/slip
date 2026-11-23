#include "side.h"
// digest-kinds §11.1 property 4: Kiln computes a class's key function when
// the class's definition ends (kiln_sema class/virtual/complete.rs), so a
// use does not make it appear: both sides have it, equal.
struct S { virtual void f(); virtual void g() {} };
#ifdef B
void h() { S s; s.g(); }
#endif
