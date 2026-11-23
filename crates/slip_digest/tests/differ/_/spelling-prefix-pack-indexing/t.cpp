#include "side.h"
// §4.4 "Spellings", rule 11: a pack-indexing prefix (`Ts...[0]::`) in a body, the index instantiation-dependent, `f` against `h`.
struct S { static constexpr int value = 1; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class... Ts> int g(Ts... ts) { return Ts...[sizeof(sizeof(h(ts...))) - 8]::value; }
#else
template<class... Ts> int g(Ts... ts) { return Ts...[sizeof(sizeof(f(ts...))) - 8]::value; }
#endif
