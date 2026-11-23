// WP-104 (digest §9), F2: what __PRETTY_FUNCTION__ prints beyond names (qualified)
namespace legacy { struct W {}; const char *f(W) { return __PRETTY_FUNCTION__; } }
