#include "side.h"
// §5.5 EnumData `int_type`: an unfixed enumeration with a negative value (int) against one without (unsigned int); the promotion type is int on both sides.
#ifdef B
enum E { a = 1 };
#else
enum E { a = -1 };
#endif
