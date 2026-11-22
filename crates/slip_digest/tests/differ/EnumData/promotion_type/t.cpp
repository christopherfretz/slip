#include "side.h"
// §5.5 EnumData `promotion_type`: an unfixed enumeration whose values need a long.
#ifdef B
enum E { a = 0x100000000 };
#else
enum E { a = 1 };
#endif
