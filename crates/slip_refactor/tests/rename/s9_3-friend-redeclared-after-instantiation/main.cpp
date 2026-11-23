namespace n { template <class T> struct S { friend void f(S) {} };
S<int> s; void f(S<int>); void u() { f(s); } }
