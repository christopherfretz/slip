#include "side.h"
// §10.1 last paragraph (WP-100 decision 4): the attribute on the defining pattern's parameter; the explicit instantiation's parameter is written as Kiln instantiated it, with it since Kiln WP-3407 (`14b25bd70`, WP-163; without it before), as clang 19 has it (unit test `attrs::tests::kiln_instantiates_parameter_attributes`).
#ifdef B
template<class T> void f(T *p);
template<class T> void f(T *p __attribute__((nonnull))) {}
template void f<int>(int *);
#else
template<class T> void f(T *p);
template<class T> void f(T *p) {}
template void f<int>(int *);
#endif
