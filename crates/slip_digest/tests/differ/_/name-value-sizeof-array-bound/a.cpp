// WP-104 (digest §9), consumed: sizeof as an array bound
namespace legacy {
unsigned long f() { char buf[sizeof(__PRETTY_FUNCTION__)]; return sizeof buf; }
}
