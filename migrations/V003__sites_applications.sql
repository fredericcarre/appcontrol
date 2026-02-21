-- V003: Sites and Applications

CREATE TABLE sites (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name VARCHAR(200) NOT NULL,
    code VARCHAR(20) NOT NULL,
    site_type VARCHAR(20) NOT NULL DEFAULT 'primary'
        CHECK (site_type IN ('primary', 'dr', 'staging', 'development')),
    location VARCHAR(200),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, code)
);

CREATE TABLE applications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    name VARCHAR(200) NOT NULL,
    description TEXT,
    site_id UUID NOT NULL REFERENCES sites(id),
    tags JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(organization_id, name)
);

CREATE INDEX idx_sites_org ON sites (organization_id);
CREATE INDEX idx_applications_org ON applications (organization_id);
CREATE INDEX idx_applications_site ON applications (site_id);
