#include "side.h"
// §4.3 `Enum.0`: which enumeration.
enum E { e }; enum F { f };
#ifdef B
F v;
#else
E v;
#endif
