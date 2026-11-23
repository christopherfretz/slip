namespace n { template <class T> struct S { friend void f(S) {} };
void f(S<int>); S<int> s; void u() { f(s); } }
