#include "side.h"
// §5.5 EnumData `flags-scoped`: scoped against unscoped, both fixed to int and empty.
#ifdef B
enum class E : int {};
#else
enum E : int {};
#endif
