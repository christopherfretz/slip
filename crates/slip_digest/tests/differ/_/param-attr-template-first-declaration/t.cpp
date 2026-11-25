#include "side.h"
// §10.1, §5.4 ParamData (WP-100 decision 4): a function template's parameter chain is over its declarations' patterns; the explicit instantiation's parameter is written as Kiln instantiated it, with the attribute the definition inherited (Kiln WP-2476; an `inherited` copy since Kiln WP-3407, written for a declaration made by instantiation, `attrs::param_nodes`, WP-163; unit test `attrs::tests::kiln_instantiates_parameter_attributes`).
#ifdef B
template<class T> void f(T *p __attribute__((nonnull)));
template<class T> void f(T *p) {}
template void f<int>(int *);
#else
template<class T> void f(T *p);
template<class T> void f(T *p) {}
template void f<int>(int *);
#endif
