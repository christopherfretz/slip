#include "side.h"
// WP-163 fix round 2 (B1): a variable template's partial specialisation's own arguments, n::k against n::j, both 0; read as `converted-arg-partial-own`.
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, int I> constexpr int v = 0;
template <class T> constexpr int v<T, n::j> = 1;
#else
template <class T, int I> constexpr int v = 0;
template <class T> constexpr int v<T, n::k> = 1;
#endif
