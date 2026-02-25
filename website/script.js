/* Context Pilot — Landing Page Scripts */

// Copy install command
function copyInstall() {
    const cmd = 'git clone https://github.com/bigmoostache/context-pilot && cd context-pilot && ./deploy_local.sh';
    navigator.clipboard.writeText(cmd).then(() => {
        const btns = document.querySelectorAll('.copy-btn');
        btns.forEach(btn => {
            const original = btn.innerHTML;
            btn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#34d399" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
            setTimeout(() => { btn.innerHTML = original; }, 2000);
        });
    });
}

// Scroll-triggered fade-in animations
const observerOptions = {
    threshold: 0.1,
    rootMargin: '0px 0px -60px 0px'
};

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        if (entry.isIntersecting) {
            entry.target.classList.add('visible');
            observer.unobserve(entry.target);
        }
    });
}, observerOptions);

document.addEventListener('DOMContentLoaded', () => {
    // Animate sections on scroll
    const animateTargets = document.querySelectorAll(
        '.problem-card, .use-case, .bento-card, .step, .arch-item, .faq-item, .stat'
    );
    animateTargets.forEach(el => {
        el.classList.add('fade-in');
        observer.observe(el);
    });

    // Nav background on scroll
    const nav = document.querySelector('.nav');
    window.addEventListener('scroll', () => {
        if (window.scrollY > 100) {
            nav.style.background = 'rgba(10, 10, 18, 0.95)';
        } else {
            nav.style.background = 'rgba(10, 10, 18, 0.85)';
        }
    });
});

// CSS for animations (injected to keep style.css clean)
const style = document.createElement('style');
style.textContent = `
    .fade-in {
        opacity: 0;
        transform: translateY(20px);
        transition: opacity 0.6s ease, transform 0.6s ease;
    }
    .fade-in.visible {
        opacity: 1;
        transform: translateY(0);
    }
    .fade-in:nth-child(2) { transition-delay: 0.1s; }
    .fade-in:nth-child(3) { transition-delay: 0.2s; }
    .fade-in:nth-child(4) { transition-delay: 0.3s; }
`;
document.head.appendChild(style);
