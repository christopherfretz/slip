#include "side.h"
#include "new_part.h"
#include "old_part.h"
#ifdef B
int u() { return acme::v + acme::g(); }
#else
int u() { return acme::v + legacy::g(); }
#endif
