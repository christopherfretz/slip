// WP-104 (digest §9), consumed: sizeof as a constexpr initialiser
namespace legacy {
unsigned long f() { constexpr unsigned long n = sizeof(__PRETTY_FUNCTION__); return n; }
}
