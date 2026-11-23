#include "side.h"
// digest-kinds §11.4: a default argument of a function template's
// specialisation, declared on both sides.
template <class T> void g(T, int a = sizeof(T));
void k() { g(1L, 0); }
#ifdef B
void h() { g(1L); }
#endif
