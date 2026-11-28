#include "side.h"
// §4.4 "Spellings", §9.5 `oper_lookups` (WP-138): unary left fold in a
// trailing return type reads its definition-context operator set.
namespace N { struct X {}; }
int operator+(N::X, int);
#ifdef B
long operator+(N::X, long);
#endif
template<class... T> auto g(T... t) -> decltype((... + t));
