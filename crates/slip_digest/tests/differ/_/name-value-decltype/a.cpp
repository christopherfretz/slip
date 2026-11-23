// WP-104 (digest §9), consumed: decltype
namespace legacy {
unsigned long f() { return sizeof(decltype(__PRETTY_FUNCTION__)); }
}
