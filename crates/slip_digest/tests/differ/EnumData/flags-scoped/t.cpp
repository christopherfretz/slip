#include "side.h"
// §5.5 EnumData `flags-scoped`: scoped against unscoped.
#ifdef B
enum class E { a };
#else
enum E { a };
#endif
