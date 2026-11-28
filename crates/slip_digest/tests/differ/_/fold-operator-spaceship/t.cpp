#include "side.h"
// WP-138 item 1: a relational comparison fold (`<`) records C++20 rewritten
// `operator<=>` candidates from the definition context (`oper_lookups`).
namespace N { struct X {}; }
#ifdef B
int operator<=>(N::X, N::X);
#endif
template<class... T> bool f(T... t) { return (t < ...); }
template<class... T> auto g(T... t) -> decltype((... < t));
