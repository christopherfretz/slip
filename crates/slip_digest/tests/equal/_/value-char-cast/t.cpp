#include "side.h"
// digest §7: `'a'` against `(char)97`.
template <auto V> struct A;
#ifdef B
A<(char)97>* x;
#else
A<'a'>* x;
#endif
