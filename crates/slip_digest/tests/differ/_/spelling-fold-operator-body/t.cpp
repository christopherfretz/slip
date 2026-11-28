#include "side.h"
// §4.4 "Spellings", §9.5 `oper_lookups` (WP-138): binary right fold in a
// body reads its definition-context operator set from `oper_lookups`.
namespace N { struct X {}; }
int operator+(N::X, int);
#ifdef B
long operator+(N::X, long);
#endif
template<class... T> auto g(T... t) { return (t + ... + 0L); }
