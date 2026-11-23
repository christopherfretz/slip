#include "side.h"
// digest-kinds §11.1 property 6: Kiln declares the inheriting
// constructor on first use, with both cold entries.
struct B0 { B0(int); };
struct D : B0 { using B0::B0; };
#ifdef B
void h() { D d(1); }
#endif
