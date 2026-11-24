namespace a { const char *longer() { return __func__; } }
const char *u() { return a::longer(); }
