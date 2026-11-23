#include "user_types.h"
#include "dep.h"
int user() { try { legacy::fail(); } catch (legacy_cfg::data_t& e) { return e.v; } catch (...) { return -1; } return 0; }
