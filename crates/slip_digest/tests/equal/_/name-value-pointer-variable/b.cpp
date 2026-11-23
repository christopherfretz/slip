// WP-104 (digest §9), bound to a pointer variable
void take(const char *);
namespace acme {
void f() { const char *p = __func__; const char *q = __PRETTY_FUNCTION__; auto r = __FUNCTION__; take(p); take(q); take(r); }
}
