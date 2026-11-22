#include "side.h"
// §8 Scalar `Null.ty`: a null `int *` against a null `char *`.
template <auto V> struct A;
#ifdef B
A<(char*)nullptr>* x;
#else
A<(int*)nullptr>* x;
#endif
