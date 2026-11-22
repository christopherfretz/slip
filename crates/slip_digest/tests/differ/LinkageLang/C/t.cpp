#include "side.h"
// §5.4 LinkageLang `C`: the enclosing language of a static function.
#ifdef B
extern "C" { static void f(); }
#else
static void f();
#endif
