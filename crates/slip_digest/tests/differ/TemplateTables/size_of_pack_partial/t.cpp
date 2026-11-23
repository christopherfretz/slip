#include "side.h"
// §9.5 TemplateTables `size_of_pack_partial`: the partly substituted arguments
// of a `sizeof...` (an alias template expanded with a pack expansion). One
// argument against two before the pack: `1 + sizeof...(U)` against
// `2 + sizeof...(U)`, two profiles, so two functions. The row itself is
// written by the full walk only, which no pair test reaches (a type is in
// profile mode, where the row is left out): expr_dep_tests::
// size_of_pack_partial_arguments_are_written.
template <unsigned long N> struct Z {};
template <class... T> using A = Z<sizeof...(T)>;
#ifdef B
template <class... U> void g(A<int, int, U...>);
#else
template <class... U> void g(A<int, U...>);
#endif
