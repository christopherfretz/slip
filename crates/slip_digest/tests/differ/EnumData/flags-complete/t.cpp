#include "side.h"
// §5.5 EnumData `flags-complete`: declared against defined.
#ifdef B
enum E : int {};
#else
enum E : int;
#endif
