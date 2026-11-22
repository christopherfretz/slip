#include "side.h"
// §5.4 VarSpec `0-inline-implied`: a constexpr static member is inline without the keyword.
#ifdef B
struct C { static constexpr int x = 1; };
#else
struct C { static inline constexpr int x = 1; };
#endif
