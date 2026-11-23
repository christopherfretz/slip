// WP-104 (digest §9), an assert-like macro in moved code
void fail(const char *, const char *, unsigned, const char *);
#define ASSERT(c) ((c) ? (void)0 : fail(#c, __FILE__, __LINE__, __PRETTY_FUNCTION__))
namespace acme {
int f(int x) { ASSERT(x > 0); return x; }
}
