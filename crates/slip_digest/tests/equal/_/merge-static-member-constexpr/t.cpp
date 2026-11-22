// std: c++17
#include "side.h"
// §2.3, §5.4 VarSpec `0-inline-implied`: an out-of-line redeclaration of an inline static member.
#ifdef B
struct C { static constexpr int x = 1; }; constexpr int C::x;
#else
struct C { static constexpr int x = 1; };
#endif
