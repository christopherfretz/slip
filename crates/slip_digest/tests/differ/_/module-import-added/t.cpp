#include "side.h"
// module: first=first.cppm
// module: second=second.cppm
// §10.7 UnitInfo `imports`, digest §2.1: a second import is a difference of the order unit.
import first;
#ifdef B
import second;
#endif
