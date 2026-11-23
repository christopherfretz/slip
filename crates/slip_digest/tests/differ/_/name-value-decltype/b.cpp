// WP-104 (digest §9), consumed: decltype
namespace acme {
unsigned long f() { return sizeof(decltype(__PRETTY_FUNCTION__)); }
}
