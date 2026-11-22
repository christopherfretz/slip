#include "side.h"
// §9.5 TemplateTables `oper_lookups`: a dependent operator expression
// keeps the non-member operators unqualified lookup found at the
// definition ([temp.dep.candidate]/1). The operator is declared before the
// template in one source and after it in the other; it is not found by
// argument-dependent lookup (`Z` is in `N`, the operator global).
namespace N { struct Z {}; }
#ifdef B
template <class T> T x{};
template <class T> auto y = x<T> + x<T>;
N::Z operator+(N::Z, N::Z);
#else
N::Z operator+(N::Z, N::Z);
template <class T> T x{};
template <class T> auto y = x<T> + x<T>;
#endif
