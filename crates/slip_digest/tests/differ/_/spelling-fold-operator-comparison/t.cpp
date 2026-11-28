#include "side.h"
// §4.4 "Spellings", §9.5 `oper_lookups` (WP-138): a fold over `!=` records
// C++20 rewritten `operator==` candidates found in the definition context.
namespace N { struct X {}; }
#ifdef B
bool operator==(N::X, long);
#endif
template<class... T> auto g(T... t) -> decltype((t != ... != 0));
