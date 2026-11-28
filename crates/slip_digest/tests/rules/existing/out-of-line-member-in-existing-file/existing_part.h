#include "side.h"
namespace acme { int x() { return 1; } }
#ifdef B
int acme::C::m() { return 2; }
#else
int legacy::C::m() { return 2; }
#endif
