#include "side.h"
// WP-035, digest-kinds §7 `ColdExprData` `default_arg_rewritten`: the default member initialiser binds a temporary to a reference member: aggregate initialisation extends it (CWG1815), so the use is rebuilt with temporaries of its own (`kiln_sema` class/temporary.rs:565).
struct Agg { const int &r = 1; };
#ifdef B
Agg v{1};
#else
Agg v{};
#endif
