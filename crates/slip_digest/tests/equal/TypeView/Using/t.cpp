#include "side.h"
// §4.3 TypeView `Using`: sugar.
namespace n { struct S {}; } using n::S;
#ifdef B
S v;
#else
n::S v;
#endif
