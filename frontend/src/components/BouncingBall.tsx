import React, { useRef, useState, useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

const BasketballSVG: React.FC<{ size?: number }> = ({ size = 52 }) => (
  <svg width={size} height={size} viewBox="0 0 52 52" fill="none" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <radialGradient id="ball-grad" cx="35%" cy="30%" r="65%">
        <stop offset="0%"   stopColor="#FF9A5C" />
        <stop offset="50%"  stopColor="#FF6B35" />
        <stop offset="100%" stopColor="#C94B1B" />
      </radialGradient>
      <radialGradient id="ball-shine" cx="35%" cy="25%" r="45%">
        <stop offset="0%"   stopColor="rgba(255,255,255,0.35)" />
        <stop offset="100%" stopColor="transparent" />
      </radialGradient>
    </defs>
    <circle cx="26" cy="26" r="25" fill="url(#ball-grad)" />
    <circle cx="26" cy="26" r="25" fill="url(#ball-shine)" />
    <path d="M26 1 C26 1 10 13 10 26 C10 39 26 51 26 51" stroke="#1a1a1a" strokeWidth="1.8" fill="none" strokeOpacity="0.7" />
    <path d="M26 1 C26 1 42 13 42 26 C42 39 26 51 26 51" stroke="#1a1a1a" strokeWidth="1.8" fill="none" strokeOpacity="0.7" />
    <path d="M1 26 C13 20 39 20 51 26" stroke="#1a1a1a" strokeWidth="1.8" fill="none" strokeOpacity="0.7" />
    <path d="M2 18 C14 28 38 28 50 18" stroke="#1a1a1a" strokeWidth="1.2" fill="none" strokeOpacity="0.4" />
    <path d="M2 34 C14 24 38 24 50 34" stroke="#1a1a1a" strokeWidth="1.2" fill="none" strokeOpacity="0.4" />
  </svg>
);

const BouncingBall: React.FC = () => {
  const ballRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const [pos, setPos] = useState({ x: window.innerWidth - 90, y: window.innerHeight - 120 });
  const [isDragging, setIsDragging] = useState(false);
  const [isBouncing, setIsBouncing] = useState(true);
  const [showTooltip, setShowTooltip] = useState(false);
  const dragOffset = useRef({ x: 0, y: 0 });
  const dragMoved = useRef(false);
  const bounceTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    setIsBouncing(false);
    dragMoved.current = false;
    dragOffset.current = { x: e.clientX - pos.x, y: e.clientY - pos.y };
  }, [pos]);

  const handleTouchStart = useCallback((e: React.TouchEvent) => {
    const touch = e.touches[0];
    setIsDragging(true);
    setIsBouncing(false);
    dragMoved.current = false;
    dragOffset.current = { x: touch.clientX - pos.x, y: touch.clientY - pos.y };
  }, [pos]);

  useEffect(() => {
    if (!isDragging) return;

    const onMouseMove = (e: MouseEvent) => {
      dragMoved.current = true;
      setPos({
        x: Math.max(0, Math.min(window.innerWidth - 52, e.clientX - dragOffset.current.x)),
        y: Math.max(0, Math.min(window.innerHeight - 52, e.clientY - dragOffset.current.y)),
      });
    };

    const onTouchMove = (e: TouchEvent) => {
      const touch = e.touches[0];
      dragMoved.current = true;
      setPos({
        x: Math.max(0, Math.min(window.innerWidth - 52, touch.clientX - dragOffset.current.x)),
        y: Math.max(0, Math.min(window.innerHeight - 52, touch.clientY - dragOffset.current.y)),
      });
    };

    const onUp = () => {
      setIsDragging(false);
      if (!dragMoved.current) {
        // It was a click, not a drag — navigate
        navigate('/how-it-works');
      }
      if (bounceTimeout.current) clearTimeout(bounceTimeout.current);
      bounceTimeout.current = setTimeout(() => setIsBouncing(true), 1500);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('touchmove', onTouchMove, { passive: true });
    window.addEventListener('touchend', onUp);

    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onUp);
      window.removeEventListener('touchmove', onTouchMove);
      window.removeEventListener('touchend', onUp);
    };
  }, [isDragging, navigate]);

  return (
    <div
      ref={ballRef}
      className={`bouncing-ball ${isBouncing && !isDragging ? 'ball-bounce' : ''}`}
      style={{
        left: pos.x,
        top: pos.y,
        transform: isDragging ? 'scale(1.15)' : undefined,
        transition: isDragging ? 'transform 0.1s ease' : undefined,
      }}
      onMouseDown={handleMouseDown}
      onTouchStart={handleTouchStart}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <BasketballSVG size={52} />

      {/* Tooltip */}
      {showTooltip && !isDragging && (
        <div style={{
          position: 'absolute',
          bottom: '110%',
          left: '50%',
          transform: 'translateX(-50%)',
          background: 'rgba(6,6,16,0.95)',
          border: '1px solid rgba(255,107,53,0.4)',
          borderRadius: 8,
          padding: '6px 12px',
          whiteSpace: 'nowrap',
          fontSize: '0.72rem',
          fontWeight: 600,
          color: 'var(--orange-bright)',
          pointerEvents: 'none',
          boxShadow: '0 4px 16px rgba(255,107,53,0.2)',
        }}>
          How the model works →
        </div>
      )}
    </div>
  );
};

export default BouncingBall;
