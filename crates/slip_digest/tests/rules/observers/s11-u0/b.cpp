#include "d.h"
namespace fresh { inline int f0() { return 0; } }
int r = fresh::f0() + old::hook(1);
