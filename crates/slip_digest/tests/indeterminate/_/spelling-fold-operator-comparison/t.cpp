#include "side.h"
// §4.4 "Spellings", rule 9: a fold over `!=` with only an `operator==` declared: C++20 rewrites `!=` from `==`, so the fold may find it.
namespace N { struct X {}; bool operator==(X, long); }
template<class... T> auto g(T... t) -> decltype((t != ... != 0));
