// WP-115 item 5, H1 (digest §9): the same as name-value-signature-local-enum through std::source_location::function_name(): the pretty name holds the local enum the signature of L::m mentions
#include "srcloc.h"
consteval const char *f() { enum F { X }; struct L { static consteval const char *m(F) { return std::source_location::current().function_name(); } }; return L::m(X); }
constexpr const char *G = f();
