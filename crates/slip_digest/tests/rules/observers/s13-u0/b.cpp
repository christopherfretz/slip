#include "d.h"
namespace fresh { inline int f0() { return 0; } }
int u0() { return fresh::f0() + sizeof(fresh::D); }
