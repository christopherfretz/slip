// WP-104 (digest §9), consumed: a function template deducing the bound
template <unsigned long N> unsigned long size(const char (&)[N]) { return N; }
namespace acme {
unsigned long f() { return size(__PRETTY_FUNCTION__); }
}
