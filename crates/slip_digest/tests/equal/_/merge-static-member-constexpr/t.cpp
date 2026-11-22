#include "side.h"
// §2.3, §5.4 VarSpec `0-inline-implied`: `inline` written on some declaration, the in-class one or the out-of-line one, so not implied.
#ifdef B
struct C { static inline constexpr int x = 1; };
constexpr int C::x;
#else
struct C { static constexpr int x = 1; };
inline constexpr int C::x;
#endif
