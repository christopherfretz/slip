#include "side.h"
// §7.2 BinaryOp `Cmp`.
namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0}, strong_ordering::greater{1}; }
int a = 1, b = 2;
#ifdef B
auto v = a < b;
#else
auto v = a <=> b;
#endif
