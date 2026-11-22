#include "side.h"
// §11.4 (WP-026): the default argument of an explicit instantiation definition is instantiated on use; the unit writes it absent.
template <class T> void f(T a, int b = sizeof(T)) {}
template void f<int>(int, int);
#ifdef B
void h() { f(1); }
#else

#endif
