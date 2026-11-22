#include "side.h"
// §5.5 EnumData `int_type`: fixed int against fixed long.
#ifdef B
enum E : long { a };
#else
enum E : int { a };
#endif
