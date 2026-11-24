#include "d.h"
namespace old { inline int f0() { return 0; } }
int r = old::f0() + old::hook(1);
