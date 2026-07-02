# Security Policy

## Reporting a Vulnerability

Nexora-Contracts contains Soroban smart contracts handling crowdfunding and fund management. Security vulnerabilities can have serious financial consequences. Please report them responsibly.

**Do NOT open a public issue to report a security vulnerability.**

### How to Report

- **GitHub Private Advisories**: Use [GitHub's private vulnerability reporting](https://github.com/Dreamville01/Nexora_contract/security/advisories/new)
- **Email**: Contact the maintainers listed in the repository README

### What to Include

- Description of the vulnerability and its potential impact
- Steps to reproduce
- Affected contract(s) and function(s)
- Proof-of-concept (if available)
- Suggested fix (optional)

### Response Timeline

| Stage | Timeline |
|-------|----------|
| Acknowledgement | 48 hours |
| Initial triage | 5 business days |
| Fix or mitigation | 30 days for critical issues |

### Scope

High-priority security areas for this project:

- Smart contract logic errors (Soroban/Stellar)
- Arithmetic overflow/underflow in fund calculations
- Unauthorized access to admin or contributor functions
- Reentrancy or state manipulation vulnerabilities
- Incorrect access control on contract invocations

Thank you for helping keep Nexora and its users safe.
