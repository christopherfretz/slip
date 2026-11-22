#include "side.h"
// §5.5 EnumData `promotion_type`: two unfixed enumerations whose underlying type is unsigned int, promoted to int against unsigned int.
#ifdef B
enum E { a = 0x80000000 };
#else
enum E { a = 1 };
#endif
