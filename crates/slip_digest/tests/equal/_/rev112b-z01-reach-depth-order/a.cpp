// rev-112b probe (shown with REACH_LIMIT lowered to 3): a nesting past the limit memoises its error into every default on the stack, so whether h is indeterminate depends on whether g (the deep use) is walked first
template<class T> const char *d0(T, const char *p = __builtin_FUNCTION());
template<class T> const char *d1(T t, const char *p = d0(T{}));
template<class T> const char *d2(T t, const char *p = d1(T{}));
template<class T> const char *d3(T t, const char *p = d2(T{}));
template<class T> const char *d4(T t, const char *p = d3(T{}));
void take(const char *);
void g() { take(d4(0)); }
void h() { take(d2(0)); }
