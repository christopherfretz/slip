// WP-104 (digest §9), review "not checked": __func__ in a default member initializer is outside every function (clang-19: "" and -Wpredefined-identifier-outside-function), so no owner is found and its bytes are written; equal under the relocation, used by two constructors
void take(const char *);
namespace acme {
struct S { const char *p = __func__; const char *q = __PRETTY_FUNCTION__; S() { take(p); } S(int) { take(q); } };
}
