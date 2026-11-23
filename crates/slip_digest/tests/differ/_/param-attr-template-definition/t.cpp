#include "side.h"
// §10.1 last paragraph (WP-100 decision 4): the attribute on the defining pattern's parameter; the explicit instantiation's parameter is written as Kiln instantiated it, without it (unit test `attrs::tests::kiln_does_not_instantiate_parameter_attributes`).
#ifdef B
template<class T> void f(T *p);
template<class T> void f(T *p __attribute__((nonnull))) {}
template void f<int>(int *);
#else
template<class T> void f(T *p);
template<class T> void f(T *p) {}
template void f<int>(int *);
#endif
