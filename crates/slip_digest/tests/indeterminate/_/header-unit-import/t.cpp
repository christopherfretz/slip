#include "side.h"
// §10.7 UnitInfo `imports`, digest §9: the key of an imported header unit is a path, and the path rule is not applied to it yet, so the order unit is indeterminate.
import "h.h";
int y = f();
