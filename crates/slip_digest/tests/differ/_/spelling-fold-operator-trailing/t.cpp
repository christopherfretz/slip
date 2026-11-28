#include "side.h"
// §4.4 "Spellings", §9.5 `oper_lookups` (WP-138): binary right fold in a
// trailing return type (profile mode + `own_spelling`) reads its
// definition-context operator set from `oper_lookups`.
namespace N { struct X {}; }
int operator+(N::X, int);
#ifdef B
long operator+(N::X, long);
#endif
template<class... T> auto g(T... t) -> decltype((t + ... + 0L));
