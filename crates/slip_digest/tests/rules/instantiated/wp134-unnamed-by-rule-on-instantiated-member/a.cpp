template <class T> struct W { enum { E = sizeof(T) }; };
int f() { return W<int>::E; }
