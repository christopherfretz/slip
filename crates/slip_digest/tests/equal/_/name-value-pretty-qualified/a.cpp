// WP-104 (digest §9), F2: what __PRETTY_FUNCTION__ prints beyond names (qualified)
namespace legacy { struct W {}; const char *f(::legacy::W) { return __PRETTY_FUNCTION__; } }
