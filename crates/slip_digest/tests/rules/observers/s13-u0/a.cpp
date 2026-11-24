#include "d.h"
namespace old { inline int f0() { return 0; } }
int u0() { return old::f0() + sizeof(fresh::D); }
