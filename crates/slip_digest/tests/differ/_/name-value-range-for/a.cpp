// WP-104 (digest §9), consumed: a range-for over __func__
int f(int) { int n = 0; for (char c : __func__) n += c != 0; return n; }
