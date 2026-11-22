#include "side.h"
// §9.5 TemplateTables `size_of_pack_partial`: the partly substituted arguments
// of a `sizeof...` (an alias template expanded with a pack expansion).
template <unsigned long N> struct Z {};
template <class... T> using A = Z<sizeof...(T)>;
#ifdef B
template <class... U> void g(A<long, U...>);
#else
template <class... U> void g(A<int, U...>);
#endif
