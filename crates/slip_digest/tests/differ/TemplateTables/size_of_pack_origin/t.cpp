#include "side.h"
// §9.5 TemplateTables `size_of_pack_origin`: the pack a substituted `sizeof...`
// named (a function parameter pack against a template parameter pack of the
// same length), in a signature that mangles it.
#ifdef B
template <class... T> auto f(T... a) -> char(*)[sizeof...(T)];
auto p = &f<int>;
#else
template <class... T> auto f(T... a) -> char(*)[sizeof...(a)];
auto p = &f<int>;
#endif
