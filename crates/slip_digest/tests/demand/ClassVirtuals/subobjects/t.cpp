#include "side.h"
// §10.6 ClassVirtuals `subobjects` (demand, §11.7): a construction that uses the vtable: `vtable_finals`, kiln_sema `class/virtual/used.rs:147`; every unit stays equal.
struct A { virtual void f() {} int a; }; struct S : A { void f() override {} };
#ifdef B
void y() { S s; }
#endif
