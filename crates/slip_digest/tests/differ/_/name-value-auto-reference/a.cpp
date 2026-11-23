// WP-104 (digest §9), consumed: auto &r
namespace legacy {
unsigned long f() { auto &r = __PRETTY_FUNCTION__; return sizeof r; }
}
