#include "side.h"
// digest-kinds §11.4: an inheriting constructor uses the base
// constructor's default arguments; it has no `default_argument` unit.
template <class T> struct B0 { B0(int, int b = sizeof(T)); };
struct D : B0<long> { using B0<long>::B0; };
void k() { B0<long> b(1, 2); }
#ifdef B
void h() { D d(1); }
#endif
