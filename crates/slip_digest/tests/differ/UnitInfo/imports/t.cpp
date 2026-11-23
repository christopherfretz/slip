#include "side.h"
// module: first=first.cppm
// module: second=second.cppm
// §10.7 UnitInfo `imports`: the same two imports in the other order. The order fixes that of the module initialisers, so it is a difference of the order unit.
#ifdef B
import second;
import first;
#else
import first;
import second;
#endif
